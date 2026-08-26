//! Integration tests for the supervisor's rejection and shutdown paths.
//!
//! These exercise `Supervisor` through the public API only, the same surface an
//! embedder gets. Every assertion here is decided by a channel or a state read,
//! never by elapsed wall-clock time: no test sleeps, and the only timeouts are
//! failure bounds that keep a hung path from blocking CI forever rather than
//! conditions a passing run depends on.
//!
//! Two paths named in issue #34 are deliberately absent -- "agent panics on
//! start" and "agent never signals ready". Both live inside the task
//! `Supervisor::spawn` starts, and nothing on the public API can make a healthy
//! `Agent` take them: `Agent::start` moves `Created -> Running` unconditionally,
//! so the `Failed("timeout")` and `Failed("channel closed")` arms are
//! unreachable from outside the crate. Covering them honestly needs a
//! fault-injection seam in the spawn path, which is a production change and
//! belongs in its own PR rather than being faked here.

use std::time::Duration;

use agentos_kernel::{
    AgentError, AgentSpec, AgentState, CallError, CircuitBreaker, CircuitBreakerConfig,
    CircuitState, LifecycleEvent, Supervisor,
};

/// Upper bound for "this should already have happened". Generous on purpose:
/// it only decides how fast a broken run fails, never whether a good run passes.
const NEVER_HANG: Duration = Duration::from_secs(5);

async fn spawn_ok(sup: &Supervisor, id: &str) {
    sup.spawn(AgentSpec::new(id, id))
        .await
        .unwrap_or_else(|e| panic!("spawning '{id}' should succeed, got {e}"));
}

// --- duplicate spawn -------------------------------------------------------

#[tokio::test]
async fn spawning_the_same_id_twice_is_rejected() {
    let sup = Supervisor::new();
    spawn_ok(&sup, "dup").await;

    let err = sup
        .spawn(AgentSpec::new("dup", "Duplicate"))
        .await
        .expect_err("second spawn of a live id must be rejected");

    assert!(
        matches!(err, AgentError::AlreadyRunning(ref id) if id == "dup"),
        "expected AlreadyRunning(\"dup\"), got {err:?}"
    );
}

#[tokio::test]
async fn a_rejected_duplicate_leaves_the_original_agent_untouched() {
    let sup = Supervisor::new();
    spawn_ok(&sup, "survivor").await;

    // A different name on the same id: if the guard ever moved to after the
    // insert, this call would overwrite the registered handle and the original
    // agent would be orphaned -- running, but no longer reachable or stoppable.
    let _ = sup.spawn(AgentSpec::new("survivor", "Impostor")).await;

    let handle = sup
        .get("survivor")
        .await
        .expect("the original agent must still be registered");
    assert_eq!(
        handle.spec().name,
        "survivor",
        "the rejected spawn overwrote the registered handle"
    );
    assert!(
        handle.is_running().await,
        "the original agent must still be running after a rejected duplicate"
    );
    assert_eq!(sup.list().await.len(), 1, "no second entry may be created");
}

// --- capacity --------------------------------------------------------------

#[tokio::test]
async fn spawning_past_max_agents_is_rejected() {
    let sup = Supervisor::new().with_max_agents(1);
    spawn_ok(&sup, "first").await;

    let err = sup
        .spawn(AgentSpec::new("second", "Second"))
        .await
        .expect_err("spawning past the cap must be rejected");

    assert!(
        matches!(err, AgentError::Internal(ref msg) if msg.contains("max agents")),
        "expected an Internal capacity error, got {err:?}"
    );
    assert_eq!(
        sup.list().await.len(),
        1,
        "the rejected agent must not be registered"
    );
}

#[tokio::test]
async fn removing_an_agent_frees_its_capacity_slot() {
    let sup = Supervisor::new().with_max_agents(1);
    spawn_ok(&sup, "occupant").await;
    sup.remove("occupant").await.expect("remove should succeed");

    // The cap counts registered handles, so a removed agent must give its slot
    // back. If it did not, a supervisor would degrade over its lifetime until
    // it could not spawn at all.
    sup.spawn(AgentSpec::new("newcomer", "Newcomer"))
        .await
        .expect("capacity should be free after remove");

    assert!(sup.get("occupant").await.is_none());
    assert!(sup.get("newcomer").await.is_some());
}

// --- stop and restart ------------------------------------------------------

#[tokio::test]
async fn a_clean_stop_leaves_the_agent_stopped() {
    let sup = Supervisor::new();
    spawn_ok(&sup, "stoppable").await;

    sup.stop("stoppable").await.expect("stop should succeed");

    let handle = sup.get("stoppable").await.expect("handle should remain");
    assert_eq!(
        handle.state().await,
        AgentState::Stopped,
        "a cleanly stopped agent must report Stopped, not Failed"
    );
    assert!(!handle.is_running().await);
}

#[tokio::test]
async fn stopping_an_unknown_agent_reports_not_found() {
    let sup = Supervisor::new();

    let err = sup
        .stop("never-existed")
        .await
        .expect_err("stopping an unknown id must fail");

    assert!(
        matches!(err, AgentError::NotFound(ref id) if id == "never-existed"),
        "expected NotFound, got {err:?}"
    );
}

#[tokio::test]
async fn restart_leaves_the_agent_running_under_the_same_id() {
    let sup = Supervisor::new();
    spawn_ok(&sup, "restartable").await;

    sup.restart("restartable")
        .await
        .expect("restart should succeed");

    let handle = sup
        .get("restartable")
        .await
        .expect("a restarted agent must stay registered under its id");
    assert!(
        handle.is_running().await,
        "a restarted agent must end up running again"
    );
}

#[tokio::test]
async fn shutdown_all_stops_every_registered_agent() {
    let sup = Supervisor::new();
    for id in ["a", "b", "c"] {
        spawn_ok(&sup, id).await;
    }

    sup.shutdown_all().await;

    for handle in sup.list().await {
        assert!(
            !handle.is_running().await,
            "agent '{}' still running after shutdown_all",
            handle.id
        );
    }
}

// --- lifecycle events ------------------------------------------------------

#[tokio::test]
async fn a_spawn_then_stop_emits_started_before_stopped() {
    let sup = Supervisor::new();
    spawn_ok(&sup, "observed").await;

    let started = tokio::time::timeout(NEVER_HANG, sup.recv_lifecycle())
        .await
        .expect("a Started event should already be queued")
        .expect("the lifecycle channel must stay open");
    assert!(
        matches!(started, LifecycleEvent::Started(ref id) if id == "observed"),
        "expected Started(\"observed\") first, got {started:?}"
    );

    sup.stop("observed").await.expect("stop should succeed");

    let stopped = tokio::time::timeout(NEVER_HANG, sup.recv_lifecycle())
        .await
        .expect("a Stopped event should follow the stop")
        .expect("the lifecycle channel must stay open");
    assert!(
        matches!(stopped, LifecycleEvent::Stopped(ref id) if id == "observed"),
        "expected Stopped(\"observed\") second, got {stopped:?}"
    );
}

#[tokio::test]
async fn a_rejected_spawn_emits_no_lifecycle_event() {
    let sup = Supervisor::new();
    spawn_ok(&sup, "quiet").await;

    // Drain the Started event the successful spawn produced.
    let _ = tokio::time::timeout(NEVER_HANG, sup.recv_lifecycle())
        .await
        .expect("the first spawn's Started event should be queued");

    let _ = sup.spawn(AgentSpec::new("quiet", "Duplicate")).await;

    // A rejected spawn never reached the agent task, so nothing may be
    // reported. Observers that count Started events must not see a phantom one.
    assert!(
        sup.try_recv_lifecycle().await.is_none(),
        "a rejected spawn must not emit a lifecycle event"
    );
}

// --- circuit breaker under repeated failures (bonus target in issue #34) ---

#[tokio::test]
async fn the_circuit_opens_once_failures_reach_the_threshold() {
    let breaker = CircuitBreaker::with_config(
        "flaky",
        CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        },
    );
    assert_eq!(breaker.state().await, CircuitState::Closed);

    for _ in 0..2 {
        let result: Result<(), CallError<&str>> =
            breaker.call(async { Err::<(), &str>("boom") }).await;
        assert!(matches!(result, Err(CallError::Inner("boom"))));
    }

    assert_eq!(
        breaker.state().await,
        CircuitState::Open,
        "the circuit must open once the failure threshold is reached"
    );
    assert!(!breaker.is_callable().await);
}

#[tokio::test]
async fn an_open_circuit_rejects_without_running_the_call() {
    let breaker = CircuitBreaker::with_config(
        "sealed",
        CircuitBreakerConfig {
            failure_threshold: 1,
            // Long enough that this test can never cross into HalfOpen.
            timeout: Duration::from_secs(3600),
            ..Default::default()
        },
    );
    let _: Result<(), CallError<&str>> = breaker.call(async { Err::<(), &str>("boom") }).await;
    assert_eq!(breaker.state().await, CircuitState::Open);

    // The whole point of the breaker is that the protected work stops being
    // attempted -- an open circuit that still ran the call would shed no load.
    let mut ran = false;
    let result: Result<(), CallError<&str>> = breaker
        .call(async {
            ran = true;
            Ok::<(), &str>(())
        })
        .await;

    assert!(
        matches!(result, Err(CallError::CircuitOpen(_))),
        "an open circuit must reject with CircuitOpen, got {result:?}"
    );
    assert!(!ran, "an open circuit must not execute the protected call");
}

#[tokio::test]
async fn resetting_an_open_circuit_lets_calls_through_again() {
    let breaker = CircuitBreaker::with_config(
        "recovered",
        CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_secs(3600),
            ..Default::default()
        },
    );
    let _: Result<(), CallError<&str>> = breaker.call(async { Err::<(), &str>("boom") }).await;
    assert_eq!(breaker.state().await, CircuitState::Open);

    breaker.reset().await;

    assert_eq!(breaker.state().await, CircuitState::Closed);
    let result: Result<u8, CallError<&str>> = breaker.call(async { Ok::<u8, &str>(7) }).await;
    assert!(
        matches!(result, Ok(7)),
        "a reset circuit must let calls through, got {result:?}"
    );
}
