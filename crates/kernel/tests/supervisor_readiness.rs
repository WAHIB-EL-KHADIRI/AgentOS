//! Integration tests for opt-in async readiness (#200).
//!
//! These drive `Supervisor` (and `AgentOSSystem`) through the public API only.
//! Readiness is a real public trait: `ReadyNow`, `FailReady`, and `NeverReady`
//! are genuine implementations, not test-only seams. Timeout determinism comes
//! from Tokio paused time, never wall-clock sleeps; the only real-time
//! timeouts are failure bounds that keep a hung path from blocking CI.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use agentos_kernel::{
    AgentError, AgentOSSystem, AgentReadiness, AgentSpec, AgentState, LifecycleEvent, Supervisor,
};

/// Failure bound: generous on purpose, only decides how fast a broken run fails.
const NEVER_HANG: Duration = Duration::from_secs(5);

struct ReadyNow;

#[async_trait::async_trait]
impl AgentReadiness for ReadyNow {
    async fn wait_until_ready(&self, _spec: &AgentSpec) -> Result<(), String> {
        Ok(())
    }
}

struct FailReady(&'static str);

#[async_trait::async_trait]
impl AgentReadiness for FailReady {
    async fn wait_until_ready(&self, _spec: &AgentSpec) -> Result<(), String> {
        Err(self.0.to_string())
    }
}

struct NeverReady;

#[async_trait::async_trait]
impl AgentReadiness for NeverReady {
    async fn wait_until_ready(&self, _spec: &AgentSpec) -> Result<(), String> {
        std::future::pending::<()>().await;
        Ok(())
    }
}

struct CountingReady {
    count: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl AgentReadiness for CountingReady {
    async fn wait_until_ready(&self, _spec: &AgentSpec) -> Result<(), String> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Readiness that signals when entered, then waits for an external gate.
/// Lets tests observe the pending window deterministically.
struct GateReady {
    entered_tx: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    gate: Arc<tokio::sync::Notify>,
}

impl GateReady {
    fn new() -> (
        Self,
        tokio::sync::oneshot::Receiver<()>,
        Arc<tokio::sync::Notify>,
    ) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let gate = Arc::new(tokio::sync::Notify::new());
        (
            Self {
                entered_tx: std::sync::Mutex::new(Some(tx)),
                gate: Arc::clone(&gate),
            },
            rx,
            gate,
        )
    }
}

#[async_trait::async_trait]
impl AgentReadiness for GateReady {
    async fn wait_until_ready(&self, _spec: &AgentSpec) -> Result<(), String> {
        if let Some(tx) = self.entered_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        self.gate.notified().await;
        Ok(())
    }
}

async fn recv_lifecycle_bounded(sup: &Supervisor) -> Option<LifecycleEvent> {
    tokio::time::timeout(NEVER_HANG, sup.recv_lifecycle())
        .await
        .ok()
        .flatten()
}

// --- default path unchanged -------------------------------------------------

#[tokio::test]
async fn default_spawn_remains_synchronous_and_unchanged() {
    let sup = Supervisor::new();
    let handle = sup
        .spawn(AgentSpec::new("plain", "Plain"))
        .await
        .expect("default spawn should succeed");
    assert!(handle.is_running().await);
    assert_eq!(handle.restart_count(), 0);
    assert!(sup.get("plain").await.is_some());
    assert_eq!(sup.list().await.len(), 1);

    let started = recv_lifecycle_bounded(&sup).await;
    assert!(
        matches!(started, Some(LifecycleEvent::Started(ref id)) if id == "plain"),
        "default spawn must still emit Started, got {started:?}"
    );
}

// --- success ----------------------------------------------------------------

#[tokio::test]
async fn ready_now_spawns_a_running_handle() {
    let sup = Supervisor::new();
    let handle = sup
        .spawn_with_readiness(AgentSpec::new("ready", "Ready"), ReadyNow)
        .await
        .expect("ReadyNow should spawn");
    assert!(handle.is_running().await);
    assert_eq!(handle.state().await, AgentState::Running);
    assert_eq!(handle.restart_count(), 0);
    assert!(sup.get("ready").await.is_some());
    assert_eq!(sup.list().await.len(), 1);

    let started = recv_lifecycle_bounded(&sup).await;
    assert!(
        matches!(started, Some(LifecycleEvent::Started(ref id)) if id == "ready"),
        "successful readiness must emit exactly the normal Started event, got {started:?}"
    );
}

#[tokio::test]
async fn successful_readiness_probe_is_not_rerun_by_soft_restart() {
    let sup = Supervisor::new();
    let count = Arc::new(AtomicUsize::new(0));
    let handle = sup
        .spawn_with_readiness(
            AgentSpec::new("restartable", "Restartable"),
            CountingReady {
                count: Arc::clone(&count),
            },
        )
        .await
        .expect("readiness should succeed");
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Drain Started so later assertions are not confused by it.
    let _ = recv_lifecycle_bounded(&sup).await;

    sup.restart("restartable")
        .await
        .expect("soft restart should succeed");
    for _ in 0..50 {
        if handle.restart_count() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(handle.restart_count(), 1);
    assert!(handle.is_running().await);
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "soft Restart must not rerun the readiness probe"
    );
}

// --- failure ----------------------------------------------------------------

#[tokio::test]
async fn fail_ready_returns_command_failed_and_frees_id_and_capacity() {
    let sup = Supervisor::new();
    let err = sup
        .spawn_with_readiness(
            AgentSpec::new("brittle", "Brittle"),
            FailReady("db unreachable"),
        )
        .await
        .expect_err("FailReady must reject the spawn");
    assert!(
        matches!(err, AgentError::CommandFailed(ref r) if r == "db unreachable"),
        "probe failure must surface as CommandFailed(reason), got {err:?}"
    );

    assert!(sup.get("brittle").await.is_none());
    assert!(sup.list().await.is_empty());
    assert!(
        sup.try_recv_lifecycle().await.is_none(),
        "rejected readiness must emit no lifecycle event"
    );

    // The slot is free: the same id can spawn immediately afterwards.
    sup.spawn_with_readiness(AgentSpec::new("brittle", "Brittle"), ReadyNow)
        .await
        .expect("id/capacity must be free after readiness failure");
    assert_eq!(sup.list().await.len(), 1);
}

#[tokio::test]
async fn rejected_readiness_emits_no_lifecycle_event() {
    let sup = Supervisor::new();
    let err = sup
        .spawn_with_readiness(AgentSpec::new("quiet-fail", "Quiet"), FailReady("nope"))
        .await
        .expect_err("must fail");
    assert!(matches!(err, AgentError::CommandFailed(_)));
    assert!(
        sup.try_recv_lifecycle().await.is_none(),
        "a rejected readiness spawn must not emit Started, Failed, or any event (#108)"
    );
}

// --- timeout (deterministic via paused time) ----------------------------------

#[tokio::test(start_paused = true)]
async fn never_ready_times_out_and_frees_id_and_capacity() {
    let sup = Supervisor::new();
    let sup_task = sup.clone();
    let join = tokio::spawn(async move {
        let mut spec = AgentSpec::new("slow", "Slow");
        spec.readiness_timeout_secs = 5;
        sup_task.spawn_with_readiness(spec, NeverReady).await
    });

    // Let the task reserve before advancing the clock.
    tokio::task::yield_now().await;
    assert!(
        sup.get("slow").await.is_none(),
        "pending spawn must not be visible via get"
    );
    assert!(sup.list().await.is_empty());

    tokio::time::advance(Duration::from_secs(10)).await;
    let err = join
        .await
        .expect("spawn task should complete")
        .expect_err("NeverReady must time out");
    assert!(
        matches!(err, AgentError::Timeout(ref id) if id == "slow"),
        "timeout must surface as Timeout(id), got {err:?}"
    );

    assert!(sup.get("slow").await.is_none());
    assert!(sup.list().await.is_empty());
    assert!(
        sup.try_recv_lifecycle().await.is_none(),
        "readiness timeout must emit no lifecycle event"
    );

    // Slot freed: the same id can spawn afterwards.
    sup.spawn_with_readiness(AgentSpec::new("slow", "Slow"), ReadyNow)
        .await
        .expect("id/capacity must be free after timeout");
}

// --- reservation / concurrency invariants -------------------------------------

#[tokio::test]
async fn pending_spawn_rejects_the_same_id() {
    let sup = Supervisor::new();
    let (gate_ready, entered, gate) = GateReady::new();
    let sup_task = sup.clone();
    let join = tokio::spawn(async move {
        sup_task
            .spawn_with_readiness(AgentSpec::new("gated", "Gated"), gate_ready)
            .await
    });

    entered.await.expect("readiness should be entered");
    // Pending: invisible yet reserved.
    assert!(sup.get("gated").await.is_none());
    assert!(sup.list().await.is_empty());

    let dup = sup
        .spawn(AgentSpec::new("gated", "Impostor"))
        .await
        .expect_err("normal spawn must reject a readiness-pending id");
    assert!(
        matches!(dup, AgentError::AlreadyRunning(ref id) if id == "gated"),
        "pending duplicate must be AlreadyRunning, got {dup:?}"
    );

    let dup_ready = sup
        .spawn_with_readiness(AgentSpec::new("gated", "Impostor"), ReadyNow)
        .await
        .expect_err("second readiness spawn must reject a pending id");
    assert!(
        matches!(dup_ready, AgentError::AlreadyRunning(ref id) if id == "gated"),
        "pending duplicate via readiness must be AlreadyRunning, got {dup_ready:?}"
    );

    gate.notify_one();
    join.await
        .expect("task should complete")
        .expect("gated readiness should succeed after gate opens");
    assert!(sup.get("gated").await.is_some());
    assert_eq!(sup.list().await.len(), 1);
}

#[tokio::test]
async fn pending_spawn_consumes_max_agents_capacity() {
    let sup = Supervisor::new().with_max_agents(1);
    let (gate_ready, entered, gate) = GateReady::new();
    let sup_task = sup.clone();
    let join = tokio::spawn(async move {
        sup_task
            .spawn_with_readiness(AgentSpec::new("first", "First"), gate_ready)
            .await
    });

    entered.await.expect("readiness should be entered");
    let err = sup
        .spawn(AgentSpec::new("second", "Second"))
        .await
        .expect_err("capacity must count pending spawns");
    assert!(
        matches!(err, AgentError::Internal(ref msg) if msg.contains("max agents")),
        "expected capacity Internal error, got {err:?}"
    );

    gate.notify_one();
    join.await
        .expect("task should complete")
        .expect("first spawn should succeed");
    // Success transfers the slot: still exactly one, still at capacity.
    assert_eq!(sup.list().await.len(), 1);
    let full = sup
        .spawn(AgentSpec::new("third", "Third"))
        .await
        .expect_err("capacity must still hold after success transfer");
    assert!(matches!(full, AgentError::Internal(_)));
}

#[tokio::test]
async fn success_transfers_slot_without_double_counting() {
    let sup = Supervisor::new().with_max_agents(2);
    sup.spawn(AgentSpec::new("a", "A"))
        .await
        .expect("first spawn should succeed");
    sup.spawn_with_readiness(AgentSpec::new("b", "B"), ReadyNow)
        .await
        .expect("readiness spawn should succeed");
    assert_eq!(sup.list().await.len(), 2);

    let err = sup
        .spawn(AgentSpec::new("c", "C"))
        .await
        .expect_err("at capacity after transfer");
    assert!(matches!(err, AgentError::Internal(_)));

    sup.remove("b").await.expect("remove should succeed");
    sup.spawn(AgentSpec::new("c", "C"))
        .await
        .expect("capacity must be free after remove");
    assert_eq!(sup.list().await.len(), 2);
}

#[tokio::test]
async fn cancellation_of_in_flight_spawn_frees_id_and_capacity() {
    let sup = Supervisor::new();
    let sup_task = sup.clone();
    let join = tokio::spawn(async move {
        sup_task
            .spawn_with_readiness(AgentSpec::new("cancelled", "Cancelled"), NeverReady)
            .await
    });
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;

    // Sanity: the id is reserved while in flight.
    let dup = sup
        .spawn(AgentSpec::new("cancelled", "Impostor"))
        .await
        .expect_err("id should be reserved while readiness is pending");
    assert!(matches!(dup, AgentError::AlreadyRunning(_)));

    join.abort();
    let _ = join.await;
    // Give the abort a chance to run the RAII Drop.
    tokio::task::yield_now().await;

    sup.spawn(AgentSpec::new("cancelled", "Recovered"))
        .await
        .expect("aborted readiness must free id/capacity");
    assert!(sup.get("cancelled").await.is_some());
}

// --- system forwarding --------------------------------------------------------

#[tokio::test]
async fn system_forwards_readiness_success_and_failure() {
    let system = AgentOSSystem::new();
    system
        .spawn_agent_with_readiness(AgentSpec::new("sys-ready", "Sys Ready"), ReadyNow)
        .await
        .expect("system readiness success should spawn");
    assert!(system.supervisor.get("sys-ready").await.is_some());

    let err = system
        .spawn_agent_with_readiness(
            AgentSpec::new("sys-brittle", "Sys Brittle"),
            FailReady("not ready"),
        )
        .await
        .expect_err("system must forward readiness failure");
    assert!(matches!(err, AgentError::CommandFailed(_)));
    assert!(system.supervisor.get("sys-brittle").await.is_none());
}

// --- serialization regression --------------------------------------------------

#[tokio::test]
async fn default_spec_shape_omits_readiness_timeout() {
    let spec = AgentSpec::new("ser", "Ser");
    let value = serde_json::to_value(&spec).expect("spec should serialize");
    let obj = value.as_object().expect("spec should be an object");
    assert!(
        !obj.contains_key("readiness_timeout_secs"),
        "default spec must stay byte-compatible; got {value}"
    );
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "capabilities",
            "heartbeat_timeout_secs",
            "id",
            "max_restarts",
            "name",
            "prompt"
        ],
        "default serialized shape must match the pre-#200 keys"
    );
}

#[tokio::test]
async fn legacy_spec_deserializes_with_default_readiness_timeout() {
    let legacy = serde_json::json!({
        "id": "legacy",
        "name": "Legacy",
        "prompt": "",
        "capabilities": [],
        "max_restarts": 5,
        "heartbeat_timeout_secs": 30
    });
    let spec: AgentSpec = serde_json::from_value(legacy).expect("legacy should parse");
    assert_eq!(spec.readiness_timeout_secs, 30);
}

#[tokio::test]
async fn custom_readiness_timeout_round_trips() {
    let mut spec = AgentSpec::new("custom", "Custom");
    spec.readiness_timeout_secs = 90;
    let value = serde_json::to_value(&spec).expect("should serialize");
    assert_eq!(value["readiness_timeout_secs"], 90);
    let back: AgentSpec = serde_json::from_value(value).expect("should deserialize");
    assert_eq!(back.readiness_timeout_secs, 90);
}
