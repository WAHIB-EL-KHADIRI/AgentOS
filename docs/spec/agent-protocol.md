# Agent Protocol

AgentOS agents communicate through an envelope-based protocol. The canonical
wire definition lives in `crates/bus/proto/agent_bus.proto`.

## Envelope

| Field | Description |
| --- | --- |
| `id` | Unique message id. |
| `source_agent_id` | Agent that emitted the message. |
| `target_agent_id` | Agent intended to receive the message. |
| `topic` | Routing topic such as `task.created` or `thought.recorded`. |
| `payload` | Encoded payload bytes. |
| `timestamp_ms` | Unix timestamp in milliseconds. |

## Lifecycle Topics

- `agent.started`
- `agent.stopped`
- `agent.failed`
- `thought.recorded`
- `checkpoint.created`
- `trace.forked`

## Time-Travel Semantics

Every recorded thought should include a checkpoint id. A replay engine can seek
to that checkpoint, rebuild visible state, and optionally fork a new branch with
a different prompt or tool result.

