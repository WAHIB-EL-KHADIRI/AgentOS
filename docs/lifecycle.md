# Agent lifecycle

`AgentState` is defined in `crates/kernel/src/agent.rs`. `Supervisor` owns the
running handles and controls their lifecycle in `crates/kernel/src/supervisor.rs`.
This page records the behavior implemented today; it is not a guarantee that the
lifecycle API will remain unchanged.

## States

| State | Meaning and trigger |
|---|---|
| `Created` | `Agent::new` created the agent. A readiness-pending spawn stays private and is not visible through `get`, `list`, or health. |
| `Running` | A normal spawn starts the agent synchronously. An opt-in readiness spawn reaches this state only after its readiness check succeeds. |
| `Stopped` | The agent loop handled `Stop` or `Shutdown`. `Agent::stop` also sets this state from `Running` or `Degraded`. |
| `Degraded(reason)` | `Agent::degrade(reason)` sets this state on a local `Agent`. The current supervisor does not set a running handle to `Degraded`; see [Lifecycle events](#lifecycle-events). |
| `Failed(reason)` | The running agent exhausted its restart budget, a soft restart failed, or its command channel closed. `Agent::fail(reason)` can also set a local agent to this state. |

`AgentState::is_running()` is true only for `Running`. `is_terminal()` is true
for `Stopped` and `Failed`.

## Spawn and readiness

`Supervisor::spawn` starts an agent without an asynchronous readiness check. It
returns a running handle or a spawn error.

Call `Supervisor::spawn_with_readiness` to opt in to `AgentReadiness`. The
supervisor reserves the agent ID and one capacity slot while the check runs.
The pending agent is not visible through `get`, `list`, or health. After the
check succeeds, the supervisor starts the agent and registers its handle.

`AgentSpec::readiness_timeout_secs` defaults to 30 seconds. If readiness returns
an error, the spawn returns `AgentError::CommandFailed(reason)`. If readiness
times out, it returns `AgentError::Timeout(agent_id)`. Neither result creates a
handle or emits a lifecycle event. The temporary agent is dropped, so callers
do not observe a `Failed` state for a rejected spawn. There is no
`Failed("timeout")` startup state.

## Running, restart, and stop

```mermaid
stateDiagram-v2
    [*] --> Created: Agent::new
    Created --> Running: spawn succeeds
    Created --> [*]: readiness fails or times out; spawn is rejected
    Running --> Running: soft restart succeeds
    Running --> Failed: soft restart fails
    Running --> Stopped: Stop or Shutdown
    Running --> Failed: restart limit reached
    Running --> Failed: command channel closes
    Running --> Degraded: direct Agent::degrade
    Degraded --> Stopped: Agent::stop
```

When a heartbeat becomes stale, the supervisor sends a soft `Restart` command.
The same agent task stays alive. A successful restart resets its runtime state
and uptime, then emits `Started`; it does not emit `Stopped`.

If the agent reaches `max_restarts`, the loop sets `Failed("max restart count
exceeded")` and emits `Failed`. If the command channel closes, it sets
`Failed("command channel closed")` and emits `Failed`. These failures happen
after the agent has a running handle. They are separate from readiness failures,
which reject the spawn before handle registration.

`Agent::start` can start a local `Agent` that is not already `Running`. That
method does not recreate a supervisor task after the task has stopped.

## Lifecycle events

The running loop emits `Started` when it starts and after each successful soft
restart. It emits `Stopped` for `Stop` and `Shutdown`, `Failed` when the running
loop fails, and `Heartbeat` every five seconds.

The supervisor can also emit `LifecycleEvent::Degraded` when it cannot dispatch
a stale-heartbeat restart. It emits this event once at the start of a failure
streak. This event does not change the handle's `AgentState` to
`Degraded(reason)`.

A rejected spawn emits no lifecycle event. The spawn result is the contract for
that operation; lifecycle events describe agents that became running handles.
